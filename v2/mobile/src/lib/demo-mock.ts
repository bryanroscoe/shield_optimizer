// Demo fixture layer for the mobile screenshot pipeline and browser-only dev.
//
// With VITE_DEMO=1 there is no Tauri host, so this installs a stand-in for
// `window.__TAURI_INTERNALS__`. Commands the phone shares with desktop are
// answered by the desktop fixtures (one Nvidia Shield, the real merged app
// list), so both galleries describe the same TV; only the mobile-only
// commands (wireless transport, licensing, files, imports, backups) are answered here.
//
// Wired in main.ts behind the VITE_DEMO flag and never ships in a real build.

import { handle as sharedHandle } from "../../../src/lib/demo-mock";
import type { BackupEntry, Device, FileEntry } from "./types";

const HOST = "192.168.1.42";
const PORT = 5555;
const SERIAL = `${HOST}:${PORT}`;
const DATA_DIR = "/data/user/0/com.atvoptimizer.mobile";

// A stand-in TV frame for the screenshot preview, drawn at runtime so no image ships.
function demoScreenBase64(): string {
  const canvas = document.createElement("canvas");
  canvas.width = 640;
  canvas.height = 360;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    ctx.fillStyle = "#101418";
    ctx.fillRect(0, 0, 640, 360);
    ctx.fillStyle = "#c9f24e";
    ctx.font = "bold 30px sans-serif";
    ctx.fillText("Demo TV screen", 40, 170);
    ctx.fillStyle = "#c7cdd6";
    ctx.font = "20px sans-serif";
    ctx.fillText("Fixture image, no device connected", 40, 210);
  }
  return canvas.toDataURL("image/png").split(",")[1] ?? "";
}

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

// Mock Google Play billing. The gallery shows a Play build that owns Pro.
// `?billing=free|pending|unavailable` in the URL previews the other states;
// in `free`, Buy succeeds after a short delay, as Play's sheet would.
type DemoBilling = "owned" | "free" | "pending" | "unavailable";
let demoBilling: DemoBilling = (() => {
  const wanted = typeof location === "undefined" ? null : new URLSearchParams(location.search).get("billing");
  return wanted === "free" || wanted === "pending" || wanted === "unavailable" ? wanted : "owned";
})();

function billingStatus() {
  const owned = demoBilling === "owned";
  const play =
    demoBilling === "owned"
      ? { state: "owned", acknowledged: true, verified_at_ms: Date.parse("2026-10-31T12:00:00Z"), cached: false }
      : demoBilling === "pending"
        ? { state: "pending" }
        : demoBilling === "unavailable"
          ? { state: "unavailable", reason: "Purchases aren't available in this build of the app." }
          : { state: "not_owned" };
  return {
    entitlement: owned ? "pro" : "free",
    sources: owned ? ["play"] : [],
    distribution: "play",
    license_key_entry: false,
    available: demoBilling !== "unavailable",
    play,
    offer:
      demoBilling === "unavailable"
        ? null
        : { product_id: "atvopt_pro", formatted_price: "$4.99", title: "ATV Optimizer Pro (ATV Optimizer)", name: "ATV Optimizer Pro", description: "Unlock every Pro tool." },
    support_offers:
      demoBilling === "unavailable"
        ? []
        : [
            { product_id: "atvopt_pro_support_1", formatted_price: "$9.99", title: "ATV Optimizer Pro, supporter", name: "ATV Optimizer Pro, supporter", description: "" },
            { product_id: "atvopt_pro_support_2", formatted_price: "$19.99", title: "ATV Optimizer Pro, big supporter", name: "ATV Optimizer Pro, big supporter", description: "" },
          ],
    ack_pending: false,
    product_id: "atvopt_pro",
  };
}

async function handleAsync(cmd: string, args: Record<string, unknown>): Promise<unknown> {
  if (cmd === "billing_purchase") {
    if (demoBilling === "unavailable")
      return { outcome: "unavailable", message: "Purchases aren't available in this build of the app.", status: billingStatus() };
    await new Promise((resolve) => setTimeout(resolve, 600));
    demoBilling = "owned";
    return { outcome: "purchased", message: "Pro unlocked. Thank you!", status: billingStatus() };
  }
  return handle(cmd, args);
}

let telemetryEnabled = true;

function handle(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case "get_entitlement":
      return demoBilling === "owned" ? "pro" : "free";
    case "activate_license":
      throw "License keys aren't used in the Google Play version. Use Buy Pro or Restore purchases.";
    case "billing_status":
    case "billing_refresh":
      return billingStatus();
    // Mirror the real backend gate so the Free preview reaches the paywall.
    case "prepare_optimize":
      if (demoBilling !== "owned") throw "LOCKED:optimize_wizard";
      return sharedHandle(cmd, args);
    // A Play build has no offline license behind its Pro.
    case "license_info":
      return null;
    case "wireless_discover":
      return {
        devices: [
          { name: "adb-0323220012345", friendly_name: "Living Room TV", host: HOST, port: PORT, service: "_adb._tcp" },
          { name: "adb-9AB4C21D7E03-x7Kq2b", friendly_name: "Den TV", host: "192.168.1.77", port: 37215, service: "_adb-tls-connect._tcp" },
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
      return { enabled: telemetryEnabled, available: true, notice_seen: true, support_id: telemetryEnabled ? "33333333-3333-4333-8333-333333333333" : null };
    case "set_telemetry_enabled":
      telemetryEnabled = Boolean(args.enabled);
      return { enabled: telemetryEnabled, available: true, notice_seen: true, support_id: telemetryEnabled ? "33333333-3333-4333-8333-333333333333" : null };
    case "telemetry_action":
    case "telemetry_error":
      return null;
    case "list_remote_dir":
      return sharedHandle("list_dir", { path: args.path }) as FileEntry[];
    case "pull_file": {
      const remote = String(args.remotePath ?? "");
      const name = remote.split("/").pop() ?? remote;
      return { name, path: `${DATA_DIR}/downloads/${name}`, size_bytes: 18432 };
    }
    case "take_screenshot":
      return { path: `${DATA_DIR}/screenshots/192-168-1-42-5555_2026-11-08_12-00-00.png`, base64: demoScreenBase64() };
    // Save a copy and Share hand off to Android's picker and share sheet, which the demo can't show.
    case "export_file":
      return args.mode === "share"
        ? { status: "share_opened" }
        : { status: "saved", name: "tv-screenshot-2026-11-08_12-00-00.png" };
    // The file picker is Android's own UI; the demo answers as if these were picked.
    case "import_pick":
      return args.purpose === "apk"
        ? {
            status: "picked",
            batch: "import-demo-apk",
            files: [
              { name: "SmartTube_stable_arm64-v8a.apk", size_bytes: 29_884_416 },
            ],
          }
        : { status: "picked", batch: "import-demo-upload", files: [{ name: "holiday-video.mp4", size_bytes: 148_242_432 }] };
    case "discard_import":
      return null;
    case "upload_conflicts":
      return [];
    case "upload_imported": {
      const dir = String(args.remoteDir ?? "/sdcard").replace(/\/$/, "");
      return { status: "uploaded", name: "holiday-video.mp4", remote_path: `${dir}/holiday-video.mp4`, size_bytes: 148_242_432, verified: true };
    }
    case "inspect_imported_apks":
      return {
        files: [{ name: "SmartTube_stable_arm64-v8a.apk", size_bytes: 29_884_416 }],
        abis: ["arm64-v8a"],
        abis_known: true,
        device_abis: ["arm64-v8a", "armeabi-v7a", "armeabi"],
        abi_compatible: true,
      };
    case "install_imported_apks":
      return { ok: true, message: "Success", hint: null };
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
    case "collect_diagnostics":
      // The desktop fixture, with the phone's host facts: no adb binary, and
      // this app's own log tail.
      return String(sharedHandle(cmd, args))
        .replace("- OS: macos (aarch64)", "- OS: android (aarch64)")
        .replace(
          /- adb path: .*\n- adb version: .*/,
          "- Support ID: 1234-5678\n- adb path: (not applicable: built-in wireless ADB client, no adb binary)\n- adb version: (unavailable)",
        )
        .replace("INFO atv_optimizer_lib: adb located", "INFO atv_optimizer: wireless_connect 192.168.1.42:5555 ok");
    case "read_device_logs": {
      const options = args.options as { package?: string | null; tag?: string | null };
      const pid = options.package ? 4321 : 1234;
      const tag = options.tag || "ActivityManager";
      return {
        package: options.package ?? null,
        pid: options.package ? pid : null,
        output: {
          stdout: [
            `11-08 12:00:01.204  ${pid}  ${pid} I ${tag}: Start proc com.google.android.youtube.tv for activity`,
            `11-08 12:00:01.611  ${pid}  ${pid} W ${options.tag || "PackageManager"}: Package org.example.app has no signatures that match`,
            `11-08 12:00:02.019  ${pid}  ${pid} I ${options.tag || "WindowManager"}: Display 0 configuration changed`,
            `11-08 12:00:03.442  ${pid}  ${pid} E ${options.tag || "MediaCodec"}: Codec reported err 0xffffffed`,
          ].join("\n") + "\n",
          stderr: "",
          exit_code: 0,
          termination: "completed",
        },
      };
    }
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
    invoke: (cmd: string, args: Record<string, unknown> = {}) => handleAsync(cmd, args),
    transformCallback: (cb: unknown) => cb,
    unregisterCallback: () => {},
    convertFileSrc: (path: string) => path,
  };
}
