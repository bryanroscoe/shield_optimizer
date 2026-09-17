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
    characteristics: "tv",
    // Shaped like a real ro.serialno so the Profile row renders at a realistic
    // width in the generated screenshots.
    serial_number: "0323220012345",
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
    { label: "H.264 / AVC", mime: "video/avc", advertised: true, acceleration_unknown: true, software: true },
    { label: "HEVC / H.265", mime: "video/hevc", advertised: true, acceleration_unknown: true, software: true },
    { label: "VP9", mime: "video/x-vnd.on2.vp9", advertised: true, acceleration_unknown: true, software: true },
    { label: "AV1", mime: "video/av01", advertised: true, acceleration_unknown: false, software: true },
    { label: "Dolby Vision", mime: "video/dolby-vision", advertised: true, acceleration_unknown: true, software: false },
    { label: "MPEG-2", mime: "video/mpeg2", advertised: true, acceleration_unknown: true, software: false },
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
      level: "info",
      title: "24p mode reported (23.976 Hz)",
      detail:
        "A matching display mode is available. Actual switching depends on the player, device, and display; this setting alone does not guarantee film-rate output.",
    },
    {
      level: "info",
      title: "Surround passthrough is on a manual allow-list",
      detail:
        "Android is configured with an explicit format list. Actual output still depends on the player and connected equipment.",
    },
    {
      level: "info",
      title: "Configuration, not a playback test",
      detail:
        "Codec entries describe available configuration. They do not verify runtime registration, acceleration, profiles, DRM, or playback performance.",
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
        // The demo layer stands in for a real release, notes included, so the
        // post-update "what's new" path is reachable without a GitHub call.
        current_notes:
          "Launcher switching is now fast and reliable.\n\n" +
          "### Launchers\n\n" +
          "- **Reliable switch away from the stock launcher.**\n" +
          "- It opens the new launcher on the TV the moment the switch succeeds.",
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
    // Every installed package, catalog entries included. The Health tab uses
    // this to tell a memory row that really is an installed app from a process
    // name it cannot tie to one — without it, every row reads "not a package".
    // Deliberately omits the native processes in top_memory (surfaceflinger
    // and friends), because those genuinely are not packages.
    case "list_installed_packages":
      return [
        ...apps.map((a) => ({
          package: a.package,
          system: true,
          enabled: true,
          name: a.name,
        })),
        { package: "com.netflix.ninja", system: false, enabled: true, name: "Netflix" },
        { package: "com.plexapp.android", system: false, enabled: true, name: "Plex" },
        { package: "com.disney.disneyplus", system: false, enabled: true, name: "Disney+" },
        { package: "com.spotify.tv.android", system: false, enabled: true, name: "Spotify" },
        { package: "tv.twitch.android.app", system: false, enabled: true, name: "Twitch" },
        { package: "com.nvidia.tegrazone3", system: true, enabled: true, name: "NVIDIA Games" },
        { package: "com.google.android.tvlauncher", system: true, enabled: true, name: null },
        { package: "com.nvidia.shield.remote.server", system: true, enabled: true, name: null },
        { package: "com.teamsmart.videomanager.tv", system: false, enabled: true, name: "SmartTube" },
        { package: "ca.devmesh.overseerrtv", system: false, enabled: true, name: "Overseerr (TV)" },
        { package: "org.fdroid.fdroid", system: false, enabled: true, name: "F-Droid" },
        { package: "com.android.vending", system: true, enabled: true, name: null },
        { package: "com.android.providers.media", system: true, enabled: true, name: null },
        { package: "com.nvidia.ota", system: true, enabled: false, name: null },
      ];
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
    // Process names from a memory report never consult the catalog — an
    // unverified string must not inherit a curated verdict.
    case "process_safety_info": {
      const proc = String(args.process ?? "");
      if (proc === "com.android.systemui" || proc === "com.google.android.gms") {
        return handle("safety_info", { package: proc });
      }
      return {
        kind: "unknown",
        reason:
          "This is a process name, not a verified package, so no reviewed verdict applies to it. Inspection only.",
        source: "no_record",
      };
    }
    case "safety_info": {
      // Mirrors crates/core/src/engine/safety.rs, including its precedence:
      // protected > caution > reviewed catalog > unknown. It has to, or the
      // demo layer paints a picture of the product that isn't true — the
      // catalog-blind version of this mock is what made every curated app in
      // the screenshots read "Unknown".
      const pkg = String(args.package ?? "");
      const protectedList: Record<string, string> = {
        "com.android.systemui":
          "System UI — the launcher's host process. Disabling makes the device unusable.",
        "com.google.android.gms":
          "Google Play Services. Disabling breaks every Google app + most third-party apps.",
      };
      const cautionList: Record<string, string> = {
        "com.android.providers.tv":
          "Live Channels provider — disabling breaks Watch Next / Continue Watching rows for Netflix, Apple TV, Disney+, etc. and the Live Channels app.",
        "com.android.vending":
          "Google Play Store. Disabling removes your install path for everything not yet on disk.",
      };
      if (protectedList[pkg]) {
        return { kind: "never_disable", reason: protectedList[pkg], source: "protected_list" };
      }
      if (cautionList[pkg]) {
        return { kind: "caution", reason: cautionList[pkg], source: "caution_list" };
      }
      const entry = (demoApps as AppEntry[]).find((a) => a.package === pkg);
      if (entry) {
        const detail = entry.optimize_description?.trim();
        const tail = detail ? ` ${detail}` : "";
        if (entry.risk === "safe") {
          return {
            kind: "safe",
            reason: `Reviewed for Android TV and rated safe to remove.${tail}`,
            source: "reviewed_catalog",
          };
        }
        const lead =
          entry.risk === "high"
            ? "Reviewed and rated high risk — read this before removing it."
            : entry.risk === "advanced"
              ? "Reviewed and rated advanced — for people who already know what this does."
              : "Reviewed and rated medium risk — removable, but you may notice it go.";
        return { kind: "caution", reason: `${lead}${tail}`, source: "reviewed_catalog" };
      }
      return {
        kind: "unknown",
        reason:
          "No reviewed app list covers this package, so what it does and what removing it would break are both unknown. Check it yourself before removing it.",
        source: "no_record",
      };
    }
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
        interfaces: [{ name: "eth0", rx_bytes_per_s: 11_534_336, tx_bytes_per_s: 204_800 }],
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
          termination: "completed",
          blocked: true,
          blocked_reason:
            "Refused: this command would disable or remove com.android.systemui, which is on the do-not-disable list. System UI — the launcher's host process. Disabling makes the device unusable.",
        };
      }
      return {
        stdout: demoShellOutput(command),
        stderr: "",
        exit_code: 0,
        termination: "completed",
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
    case "find_files":
      return { hits: [], unsearched: [] };
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
        settings_to_delete: ["global.encoded_surround_output"],
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
        settings_deleted: ["global.encoded_surround_output"],
        settings_failed: [],
        summary: "Applied snapshot: 2 disabled, launcher set, 2 settings written, 1 reset.",
      };
    case "prepare_optimize":
      return optimizePlan((args.mode as "optimize" | "restore") ?? "optimize");
    case "report_all":
      return [{ serial: SERIAL, name: device.name, report: health, error: null }];
    case "pair_device":
      return {
        ok: true,
        message: "Paired successfully. Pairing established trust; to connect, enter the separate IP:port shown on the TV's main Wireless debugging screen in Connect IP.",
      };
    case "connect_device":
      return { ok: true, message: `connected to ${String(args.address)}` };
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
