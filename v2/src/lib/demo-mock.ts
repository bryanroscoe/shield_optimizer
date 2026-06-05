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
import type {
  AppEntry,
  Device,
  HealthReport,
  LauncherStatus,
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

// Map of command name → handler. Unlisted commands fall through to a benign
// success so a stray click during capture never throws.
function handle(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case "adb_status":
      return { available: true, path: "/opt/homebrew/bin/adb", last_probe: "2026-06-02T14:40:00Z" };
    case "check_for_update":
      return {
        current: "0.1.0-beta.9",
        latest: "0.1.0-beta.9",
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
    case "safety_info":
      return { kind: "safe" };
    case "list_launchers":
      return launchers;
    case "current_launcher":
      return { package: "com.spocky.projengmenu", activity: "com.spocky.projengmenu/.MainActivity" };
    case "channel_provider_disabled":
      return false;
    case "get_tweaks":
      return tweaks;
    case "list_dir":
      return demoFiles(args.path as string);
    case "get_display_scaling":
      return { size: "1920x1080 (default)", density: "320 (default)" };
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
