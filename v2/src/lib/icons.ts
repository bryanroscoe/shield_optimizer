// Every Material Symbols icon the desktop app is allowed to use.
//
// This list is the single source of truth in two directions. `IconName` makes
// a typo a `npm run check` error, and scripts/subset-material-symbols.py reads
// this file to decide which glyphs to keep — the bundled font contains these
// and nothing else, which is how a 5 MB source becomes ~30 KB.
//
// Adding an icon: put the ligature name here, then run `npm run subset-icons`
// and commit the regenerated woff2. The script fails loudly if a name here is
// not a real ligature, so a misspelling cannot ship silently.
//
// Deliberately NOT icons: the arrows in prose ("Settings → Developer options")
// and the mathematical glyphs (≤ ≈ ⇒ ≠). Those are typography and an icon font
// has nothing useful to say about them.

export const ICONS = [
  // Navigation and chrome
  "arrow_back",
  "arrow_downward",
  "arrow_upward",
  "chevron_right",
  "expand_more",
  "open_in_new",
  "refresh",
  "close",
  "check",
  "add",
  "search",
  "more_vert",

  // Status and safety
  "warning",
  "error",
  "info",
  "shield",
  "help",
  "check_circle",
  "lock",

  // Device and hardware
  "tv",
  "cast",
  "memory",
  "storage",
  "device_thermostat",
  "monitor_heart",
  "speed",
  "aspect_ratio",
  "volume_up",
  "volume_down",
  "volume_off",
  "power_settings_new",
  "restart_alt",
  "photo_camera",

  // Tabs and sections
  "dashboard",
  "auto_fix_high",
  "home",
  "apps",
  "tune",
  "history",
  "folder",
  "folder_open",
  "android",
  "image",
  "upload",
  "description",
  "link",
  "download",
  "shop",
  "terminal",
  "play_circle",
  "settings_remote",
  "settings",

  // Remote control
  "keyboard_arrow_up",
  "keyboard_arrow_down",
  "keyboard_arrow_left",
  "keyboard_arrow_right",
  "play_pause",
  "play_arrow",
  "fast_forward",
  "fast_rewind",
  "keyboard_command_key",

  // Actions
  "save",
  "delete",
  "content_copy",
  "content_paste",
  "swap_horiz",
  "restore",
  "local_cafe",
] as const;

export type IconName = (typeof ICONS)[number];
