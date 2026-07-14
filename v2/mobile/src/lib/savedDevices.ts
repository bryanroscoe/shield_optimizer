// Saved-TV persistence for the design's §1.0 Reconnect flow. Paired TVs are
// remembered in localStorage (host/port/name/type + last-used time) so the app
// can offer a one-tap "Reconnect to <name>" on launch instead of always
// re-scanning. The RSA pairing key is persisted Kotlin-side, so reconnecting a
// previously-authorized TV is silent — this is only app-side bookkeeping.
//
// This never holds secrets: no keys, no PINs. Just enough to re-open the ADB
// socket to a TV the phone already trusts.

import type { Device, SavedDevice } from "./types";
import { deviceLabelOf } from "./types";

const KEY = "atv.savedDevices.v1";
const MAX = 8;
const EPOCH = new Date(0).toISOString();

function normalizeSavedDevice(value: unknown): SavedDevice | null {
  if (!value || typeof value !== "object") return null;
  const d = value as Record<string, unknown>;
  if (
    typeof d.host !== "string" ||
    d.host.trim() === "" ||
    typeof d.connectPort !== "number" ||
    !Number.isInteger(d.connectPort) ||
    d.connectPort < 1 ||
    d.connectPort > 65535
  ) {
    return null;
  }
  const deviceType =
    d.deviceType === "shield" ||
    d.deviceType === "google_tv" ||
    d.deviceType === "unknown"
      ? d.deviceType
      : d.deviceType === "googletv"
        ? "google_tv"
        : "unknown";
  const parsedLastUsed =
    typeof d.lastUsed === "string" ? new Date(d.lastUsed) : new Date(NaN);
  return {
    host: d.host.trim(),
    connectPort: d.connectPort,
    name:
      typeof d.name === "string" && d.name.trim() !== ""
        ? d.name.trim()
        : d.host.trim(),
    deviceType,
    lastUsed: Number.isNaN(parsedLastUsed.getTime())
      ? EPOCH
      : parsedLastUsed.toISOString(),
  };
}

function read(): SavedDevice[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Normalize older or partially-corrupt records in memory. Invalid dates
    // sort to the end instead of throwing during reconnect-screen startup.
    return parsed
      .map(normalizeSavedDevice)
      .filter((d): d is SavedDevice => d !== null);
  } catch {
    return [];
  }
}

function write(list: SavedDevice[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(list.slice(0, MAX)));
  } catch {
    // Storage unavailable/full — reconnect simply won't be offered next launch.
  }
}

/// All saved TVs, most-recently-used first.
export function listSavedDevices(): SavedDevice[] {
  return read().sort((a, b) => b.lastUsed.localeCompare(a.lastUsed));
}

/// The single most-recently-used TV, or null. Drives the §1.0 landing screen.
export function lastSavedDevice(): SavedDevice | null {
  return listSavedDevices()[0] ?? null;
}

/// Record (or refresh) a successful connection. Keyed by host:port so a TV that
/// moves ports gets a fresh row rather than a stale duplicate.
export function rememberDevice(
  host: string,
  connectPort: number,
  device: Device | null,
): void {
  const list = read().filter(
    (d) => !(d.host === host && d.connectPort === connectPort),
  );
  list.unshift({
    host,
    connectPort,
    name: deviceLabelOf(device),
    deviceType: device?.device_type ?? "unknown",
    lastUsed: new Date().toISOString(),
  });
  write(list);
}

export function forgetDevice(host: string, connectPort: number): void {
  write(read().filter((d) => !(d.host === host && d.connectPort === connectPort)));
}

/// Compact "last used" phrasing for the reconnect card (e.g. "2h ago").
export function lastUsedLabel(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const mins = Math.max(0, Math.round((Date.now() - then) / 60000));
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.round(hrs / 24);
  return `${days}d ago`;
}
