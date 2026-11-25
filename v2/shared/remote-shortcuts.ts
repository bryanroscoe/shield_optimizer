// The Remote's Apps key: installed apps by name, and up to three pinned
// shortcuts per TV. Both apps use this module so a TV's pins mean the same
// thing on either.

export const MAX_PINS = 3;

/// The parts of a `list_launchable_apps` row this needs.
export interface LaunchableRow {
  package: string;
  name: string | null;
}

/// What a row is called on screen: its name, or its package id when no list
/// knows it. Never a logo or other artwork.
export function appLabel(app: LaunchableRow): string {
  return app.name?.trim() || app.package;
}

/// Case-insensitive match on the name and the package id.
export function filterApps<T extends LaunchableRow>(apps: readonly T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...apps];
  return apps.filter((a) => appLabel(a).toLowerCase().includes(q) || a.package.toLowerCase().includes(q));
}

const STORAGE_KEY = "atvopt.remotePins";

/// The key a TV's pins are filed under: its verified hardware id, never its
/// address, because whatever answers there later may be another TV. A TV
/// with no readable id has no key; its pins last only while the screen is open.
function idKey(hardwareId: string | null | undefined): string | null {
  const id = hardwareId?.trim();
  return id && id.toLowerCase() !== "unknown" ? id : null;
}

const PACKAGE = /^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$/;

function readAll(storage: Storage): Record<string, string[]> {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(STORAGE_KEY) ?? "{}");
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    const out: Record<string, string[]> = {};
    for (const [id, pins] of Object.entries(parsed as Record<string, unknown>)) {
      if (!Array.isArray(pins)) continue;
      const clean = [...new Set(pins.filter((p): p is string => typeof p === "string" && PACKAGE.test(p)))];
      if (clean.length) out[id] = clean.slice(0, MAX_PINS);
    }
    return out;
  } catch {
    return {};
  }
}

export function pinnedApps(storage: Storage, hardwareId: string | null | undefined): string[] {
  const key = idKey(hardwareId);
  return key ? (readAll(storage)[key] ?? []) : [];
}

export type PinResult = {
  /// The pins after the change, in the order they were pinned.
  pins: string[];
  /// False when the TV has no id or storage refused the write: the change
  /// holds for the open screen only.
  saved: boolean;
};

/// Pins or unpins `pkg` for this TV, starting from `current` (what the screen
/// shows). Pinning a fourth app is refused: `pins` comes back unchanged.
export function togglePin(
  storage: Storage,
  hardwareId: string | null | undefined,
  current: readonly string[],
  pkg: string,
): PinResult {
  if (!PACKAGE.test(pkg)) return { pins: [...current], saved: false };
  const pins = current.includes(pkg)
    ? current.filter((p) => p !== pkg)
    : current.length >= MAX_PINS
      ? [...current]
      : [...current, pkg];
  const key = idKey(hardwareId);
  if (!key) return { pins, saved: false };
  const all = readAll(storage);
  if (pins.length) all[key] = pins;
  else delete all[key];
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(all));
    return { pins, saved: true };
  } catch {
    return { pins, saved: false };
  }
}
