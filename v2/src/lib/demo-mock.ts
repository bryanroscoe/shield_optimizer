// Demo fixture layer for screenshots and offline UI work.
//
// When the app runs with VITE_DEMO=1 there is no Tauri host, so
// `@tauri-apps/api`'s `invoke()` (which reads `window.__TAURI_INTERNALS__`)
// has nothing to talk to. This module installs a stand-in that answers every
// command with realistic data for one fictional-but-faithful Nvidia Shield —
// real package names, real launcher catalog, the real merged app list — so
// the screenshot pipeline (and any browser-only dev session) renders every
// screen without a device attached.
//
// It is wired in `+layout.ts` behind the VITE_DEMO flag and never ships in a
// real build.

import demoApps from "./demo-apps.json";
import pkg from "../../package.json";
import type {
  AppEntry,
  Device,
  HealthReport,
  LauncherStatus,
  MediaCapabilities,
  OptimizePlan,
  OptimizePlanItem,
  SnapshotFile,
  TweaksState,
} from "./types";

const SERIAL = "192.168.1.42:5555";

const device: Device = {
  id: 1,
  serial: SERIAL,
  name: "NVIDIA SHIELD Android TV",
  model: "SHIELD Android TV (2019 Pro)",
  device_type: "shield",
  status: "device",
  connection: "network",
  properties: {
    friendly_name: "NVIDIA SHIELD Android TV",
    brand: "NVIDIA",
    model: "SHIELD Android TV",
    device_codename: "mdarcy",
    manufacturer: "NVIDIA",
    android_release: "11",
    sdk_level: "30",
    build_id: "PPR1.180610.011",
    board_platform: "tegra",
  },
};

const apps = demoApps as AppEntry[];

const health: HealthReport = {
  display: {
    resolution: "3840x2160",
    refresh_hz: 60,
    hdr_types: ["HDR10", "Dolby Vision", "HLG"],
  },
  ram: { total_mb: 2956, used_mb: 1894, free_mb: 1062, swap_mb: 512 },
  storage: { total: "15G", used: "9.2G", available: "5.1G", used_percent: 64 },
  temperature_c: 47.5,
  audio_device: "Dolby Atmos over HDMI (eARC)",
  top_memory: [
    { package: "com.netflix.ninja", mb: 312 },
    { package: "com.amazon.amazonvideo.livingroom", mb: 268 },
    { package: "com.google.android.youtube.tv", mb: 241 },
    { package: "com.plexapp.android", mb: 198 },
    { package: "com.google.android.tvlauncher", mb: 176 },
    { package: "com.nvidia.tegrazone3", mb: 154 },
    { package: "com.google.android.gms", mb: 142 },
    { package: "com.disney.disneyplus", mb: 131 },
    { package: "com.spotify.tv.android", mb: 118 },
    { package: "tv.twitch.android.app", mb: 104 },
    { package: "com.android.systemui", mb: 96 },
    { package: "com.nvidia.shield.remote.server", mb: 71 },
  ],
};

const media: MediaCapabilities = {
  video: [
    { label: "H.264 / AVC", mime: "video/avc", hardware: true, software: true },
    { label: "HEVC / H.265", mime: "video/hevc", hardware: true, software: true },
    { label: "VP9", mime: "video/x-vnd.on2.vp9", hardware: true, software: true },
    { label: "AV1", mime: "video/av01", hardware: false, software: true },
    { label: "Dolby Vision", mime: "video/dolby-vision", hardware: true, software: false },
    { label: "MPEG-2", mime: "video/mpeg2", hardware: true, software: false },
  ],
  hdr_types: ["Dolby Vision", "HDR10", "HLG"],
  modes: [
    { width: 3840, height: 2160, fps: 59.94, active: true },
    { width: 3840, height: 2160, fps: 29.97, active: false },
    { width: 3840, height: 2160, fps: 23.976, active: false },
    { width: 1920, height: 1080, fps: 60.0, active: false },
    { width: 1920, height: 1080, fps: 23.976, active: false },
  ],
  audio: {
    mode: "manual",
    enabled_formats: [
      "Dolby Digital (AC-3)",
      "Dolby Digital Plus (E-AC-3)",
      "Dolby Atmos over DD+ (E-AC-3 JOC)",
      "Dolby TrueHD",
      "DTS",
      "DTS-HD",
    ],
    raw_formats: "5,6,18,14,7,8",
  },
  match_content_frame_rate: "2",
  verdicts: [
    {
      level: "good",
      title: "24p handled (23.976 Hz mode available)",
      detail:
        "Match Content Frame Rate is set to Always, so film switches to its native cadence instead of being pulled to the panel rate.",
      note: null,
    },
    {
      level: "info",
      title: "Surround passthrough is on a manual allow-list",
      detail:
        "Only these pass through: Dolby Digital (AC-3), Dolby Digital Plus (E-AC-3), Dolby Atmos over DD+ (E-AC-3 JOC), Dolby TrueHD, DTS, DTS-HD. Anything else is decoded on the device.",
      note: null,
    },
    {
      level: "good",
      title: "Dolby Vision available",
      detail:
        "The device advertises a Dolby Vision decoder and the display chain accepts Dolby Vision.",
      note: "Profiles 5 and 8 play natively. Profile 7 — the dual-layer format UHD Blu-ray remuxes use — plays the base layer only: the enhancement layer is discarded, so FEL titles render from a base grade that was never meant to be shown alone. Converting Profile 7 to 8.1 before playback avoids that.",
    },
    {
      level: "warn",
      title: "AV1 in software only",
      detail:
        "No hardware AV1 decoder is advertised. AV1 falls back to CPU decoding, which stutters above 1080p on TV-class silicon.",
      note: "The Shield's Tegra X1/X1+ has no AV1 decode block, and no firmware update can add one. AV1 streams fall back to software decoding — fine at 1080p, unreliable above it.",
    },
  ],
};

const launchers: LauncherStatus[] = [
  {
    entry: { name: "Android TV Launcher (Stock)", package: "com.google.android.tvlauncher" },
    installed: true,
    enabled: true,
    stock: true,
    other: false,
  },
  { entry: { name: "Projectivy Launcher", package: "com.spocky.projengmenu" }, installed: true, enabled: true, stock: false, other: false },
  { entry: { name: "FLauncher", package: "me.efesser.flauncher" }, installed: true, enabled: true, stock: false, other: false },
  { entry: { name: "ATV Launcher", package: "com.sweech.launcher" }, installed: false, enabled: false, stock: false, other: false },
  { entry: { name: "Wolf Launcher", package: "com.wolf.firelauncher" }, installed: false, enabled: false, stock: false, other: false },
];

const tweaks: TweaksState = {
  hdmi_control_enabled: "1",
  hdmi_control_auto_wakeup_enabled: "0",
  hdmi_control_auto_device_off_enabled: "0",
  hdmi_system_audio_control_enabled: "1",
  match_content_frame_rate: "2",
  long_press_timeout: "400",
  window_animation_scale: "0.5",
  transition_animation_scale: "0.5",
  animator_duration_scale: "0.5",
  background_process_limit: "2",
  encoded_surround_output: "3",
  encoded_surround_output_enabled_formats: "5,6,18,14,7,8",
};

const snapshots: SnapshotFile[] = [
  {
    path: "/Users/you/Library/Application Support/com.shieldoptimizer.app/snapshots/shield-living-room-2026-05-12.json",
    filename: "shield-living-room-2026-05-12.json",
    saved_at: "2026-05-12T18:42:09Z",
    label: "After debloat + Projectivy",
    device_name: "NVIDIA SHIELD Android TV",
    device_serial: SERIAL,
    device_type: "shield",
    disabled_count: 23,
    settings_count: 6,
    launcher: "com.spocky.projengmenu",
  },
  {
    path: "/Users/you/Library/Application Support/com.shieldoptimizer.app/snapshots/shield-bedroom-2026-04-28.json",
    filename: "shield-bedroom-2026-04-28.json",
    saved_at: "2026-04-28T09:15:33Z",
    label: null,
    device_name: "SHIELD (Bedroom)",
    device_serial: "192.168.1.57:5555",
    device_type: "shield",
    disabled_count: 19,
    settings_count: 4,
    launcher: "me.efesser.flauncher",
  },
];

// Deterministic per-package state so the App List screen shows a believable
// mix without random churn between renders. A few of the default-optimize
// bloat entries read as already-disabled; a couple of optional packages as
// not-installed; everything else enabled.
const DISABLED = new Set([
  "com.nvidia.stats",
  "com.nvidia.diagtools",
  "com.google.android.tvrecommendations",
  "com.amazon.amazonvideo.livingroom",
  "com.facebook.katana",
]);
const MISSING = new Set(["com.disney.disneyplus", "com.wolf.firelauncher"]);

function packageStates(packages: string[]): Record<string, "enabled" | "disabled" | "missing"> {
  const out: Record<string, "enabled" | "disabled" | "missing"> = {};
  for (const p of packages) {
    out[p] = MISSING.has(p) ? "missing" : DISABLED.has(p) ? "disabled" : "enabled";
  }
  return out;
}

function optimizePlan(mode: "optimize" | "restore"): OptimizePlan {
  const memoryByPkg: Record<string, number> = Object.fromEntries(
    health.top_memory.map((m) => [m.package, m.mb]),
  );
  // Mirror the real backend (engine::compute_plan): include every installed
  // catalog app with its natural action, regardless of default_optimize. The
  // wizard UI is what applies the per-app default (non-default apps default to
  // Skip), so the plan must carry the full set for that to be visible.
  const items: OptimizePlanItem[] = apps
    .slice(0, 16)
    .map((entry) => {
      const state = MISSING.has(entry.package)
        ? "missing"
        : DISABLED.has(entry.package)
          ? "disabled"
          : "enabled";
      let action: OptimizePlanItem["action"];
      if (mode === "optimize") {
        action =
          state === "missing"
            ? { kind: "skip", reason: "not_installed" }
            : state === "disabled"
              ? { kind: "skip", reason: "already_disabled" }
              : entry.method === "uninstall"
                ? { kind: "uninstall" }
                : { kind: "disable" };
      } else {
        action =
          state === "disabled" ? { kind: "enable" } : { kind: "skip", reason: "already_enabled" };
      }
      return { entry, action, memory_mb: memoryByPkg[entry.package] ?? null };
    });
  return { mode, items };
}

function demoFiles(path: string) {
  if (path === "/sdcard") {
    return [
      { name: "Download", is_dir: true, is_symlink: false, size_bytes: 4096, modified: "2026-05-28 19:02" },
      { name: "Movies", is_dir: true, is_symlink: false, size_bytes: 4096, modified: "2026-04-11 21:47" },
      { name: "Projectivy", is_dir: true, is_symlink: false, size_bytes: 4096, modified: "2026-05-12 18:40" },
      { name: "device-report.txt", is_dir: false, is_symlink: false, size_bytes: 18432, modified: "2026-06-01 09:15" },
      { name: "screen-test.png", is_dir: false, is_symlink: false, size_bytes: 2411724, modified: "2026-05-30 20:08" },
    ];
  }
  return [
    { name: "smarttube-backup.json", is_dir: false, is_symlink: false, size_bytes: 9216, modified: "2026-05-12 18:41" },
    { name: "wallpaper.jpg", is_dir: false, is_symlink: false, size_bytes: 1048576, modified: "2026-05-12 18:40" },
  ];
}

function demoShellOutput(command: string): string {
  if (command.includes("packages -d")) {
    return [...DISABLED].map((p) => `package:${p}`).join("\n");
  }
  if (command.includes("packages -3")) {
    return [
      "package:com.plexapp.android",
      "package:com.spocky.projengmenu",
      "package:com.liskovsoft.smarttubetv.beta",
      "package:org.jellyfin.androidtv",
    ].join("\n");
  }
  if (command.includes("getprop")) {
    return [
      "[ro.product.brand]: [NVIDIA]",
      "[ro.product.device]: [mdarcy]",
      "[ro.product.manufacturer]: [NVIDIA]",
      "[ro.product.model]: [SHIELD Android TV]",
      "[ro.product.name]: [darcy]",
    ].join("\n");
  }
  if (command.includes("uptime")) {
    return " 21:14:07 up 6 days,  3:22,  0 users,  load average: 0.84, 0.61, 0.55";
  }
  return "ok";
}

// Map of command name → handler. Unlisted commands fall through to a benign
// success so a stray click during capture never throws.
function handle(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case "adb_status":
      return { available: true, path: "/opt/homebrew/bin/adb", last_probe: "2026-06-02T14:40:00Z" };
    case "check_for_update":
      // Real version so screenshots never show a stale header badge.
      return {
        current: pkg.version,
        latest: pkg.version,
        update_available: false,
        url: "https://github.com/bryanroscoe/shield_optimizer/releases",
      };
    case "list_devices":
      return [device];
    case "device_profile":
      return device;
    case "health_report":
      return health;
    case "app_list_for_device":
      return apps;
    case "package_states":
      return packageStates((args.packages as string[]) ?? []);
    case "app_permission_state":
      return "granted";
    case "list_other_packages":
      return [
        { package: "com.teamsmart.videomanager.tv", system: false, enabled: true, name: "SmartTube" },
        { package: "ca.devmesh.overseerrtv", system: false, enabled: true, name: "Overseerr (TV)" },
        { package: "org.fdroid.fdroid", system: false, enabled: true, name: "F-Droid" },
        { package: "com.android.vending", system: true, enabled: true, name: null },
        { package: "com.android.providers.media", system: true, enabled: true, name: null },
        { package: "com.nvidia.ota", system: true, enabled: false, name: null },
      ];
    case "app_memory_map":
      return {
        "com.teamsmart.videomanager.tv": 184.2,
        "com.netflix.ninja": 243.7,
        "com.amazon.amazonvideo.livingroom.nvidia": 126.5,
        "com.spocky.projengmenu": 92.1,
      };
    case "app_usage_map":
      return {
        "com.netflix.ninja": { last_used: "2026-06-05 20:10:00", launch_count: 412 },
        "com.teamsmart.videomanager.tv": { last_used: "2026-06-04 21:30:00", launch_count: 88 },
        "com.hulu.plus": { last_used: "2026-03-12 19:02:00", launch_count: 4 },
        "com.showtime.standalone": { last_used: null, launch_count: 0 },
      };
    case "safety_info":
      return { kind: "safe" };
    case "list_launchers":
      return launchers;
    case "current_launcher":
      return { package: "com.spocky.projengmenu", activity: "com.spocky.projengmenu/.MainActivity" };
    case "channel_provider_disabled":
      return false;
    case "media_report":
      return media;
    case "resource_sample":
      return {
        cpu_percent: 18.4,
        rx_bytes_per_s: 11_534_336,
        tx_bytes_per_s: 204_800,
        interval_ms: 1000,
      };
    case "run_shell": {
      const command = String(args.command ?? "");
      // Mirror the real safety gate so the demo/screenshot layer cannot show
      // a refusal-free shell that the shipping app would never allow.
      if (/\b(disable|disable-user|uninstall|hide|suspend)\b/.test(command) &&
          /\b(android|com\.android\.systemui|com\.android\.shell|com\.google\.android\.gms)\b/.test(command)) {
        return {
          stdout: "",
          stderr: "",
          exit_code: null,
          blocked: true,
          blocked_reason:
            "Refused: this command would disable or remove com.android.systemui, which is on the do-not-disable list. System UI — the launcher's host process. Disabling makes the device unusable.",
        };
      }
      return {
        stdout: demoShellOutput(command),
        stderr: "",
        exit_code: 0,
        blocked: false,
        blocked_reason: null,
      };
    }
    case "get_tweaks":
      return tweaks;
    case "list_dir":
      return demoFiles(args.path as string);
    case "get_display_scaling":
      return { size: "1920x1080 (default)", density: "320 (default)" };
    case "get_private_dns":
      return { mode: "opportunistic", hostname: null };
    case "set_private_dns":
      return { ok: true, message: "Private DNS updated.", reverted: false };
    case "list_snapshots":
      return snapshots;
    case "snapshot_dir_path":
      return "/Users/you/Library/Application Support/com.shieldoptimizer.app/snapshots";
    case "list_apks_in_folder":
      return [];
    case "preview_apply":
      return {
        packages_to_disable: [
          "com.google.android.feedback",
          "com.android.printspooler",
          "com.google.android.videos",
          "com.google.android.music",
        ],
        packages_already_disabled: ["com.amazon.amazonvideo.livingroom", "com.facebook.katana"],
        packages_not_installed: ["com.disney.disneyplus", "com.quibi.qlient"],
        launcher_to_set: "com.spocky.projengmenu",
        settings_to_write: {
          "global.hdmi_control_enabled": "1",
          "secure.match_content_frame_rate": "2",
          "global.window_animation_scale": "0.5",
        },
        settings_already_set: ["global.transition_animation_scale", "global.animator_duration_scale"],
        cross_device_warning: null,
      };
    case "apply_snapshot":
      return {
        packages_disabled: ["com.google.android.feedback", "com.android.printspooler"],
        packages_failed: [],
        launcher_set: true,
        launcher_message: "Set Projectivy as default.",
        settings_written: ["global.hdmi_control_enabled", "secure.match_content_frame_rate"],
        settings_failed: [],
        summary: "Applied snapshot: 2 disabled, launcher set, 2 settings written.",
      };
    case "prepare_optimize":
      return optimizePlan((args.mode as "optimize" | "restore") ?? "optimize");
    case "report_all":
      return [{ serial: SERIAL, name: device.name, report: health, error: null }];
    default:
      // Mutating commands (disable_package, set_default_launcher, …) aren't
      // exercised during capture; answer benignly just in case.
      return { ok: true, message: "demo mode — no-op" };
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
