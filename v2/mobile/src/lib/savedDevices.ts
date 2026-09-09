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
const AUTO_KEY = "atv.autoConnect.v1";
const MAX = 16;
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
  const hardwareId =
    typeof d.hardwareId === "string" && d.hardwareId.trim() !== ""
      ? d.hardwareId.trim()
      : undefined;
  return {
    host: d.host.trim(),
    connectPort: d.connectPort,
    ...(hardwareId ? { hardwareId } : {}),
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
    const newestFirst = [...list].sort((a, b) =>
      b.lastUsed.localeCompare(a.lastUsed),
    );
    localStorage.setItem(KEY, JSON.stringify(newestFirst.slice(0, MAX)));
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

function hardwareIdOf(device: Device | null): string | undefined {
  const id = device?.properties?.serial_number?.trim();
  return id && id !== "unknown" ? id : undefined;
}

/// Does a saved row describe the TV we just connected to? Hardware ids are
/// strong identity. Without them, only an unchanged endpoint is trustworthy.
function sameTv(
  row: SavedDevice,
  host: string,
  connectPort: number,
  hardwareId: string | undefined,
): boolean {
  if (hardwareId || row.hardwareId) {
    return hardwareId !== undefined && row.hardwareId === hardwareId;
  }
  return row.host === host && row.connectPort === connectPort;
}

/// Record (or refresh) a successful connection. The hardware serial is the
/// durable identity when the TV reports one (ports rotate and DHCP can hand a
/// TV's old IP to another device); the host is the fallback.
export function rememberDevice(
  host: string,
  connectPort: number,
  device: Device | null,
): void {
  const current = read();
  const hardwareId = hardwareIdOf(device);
  const existing = current.find((d) =>
    sameTv(d, host, connectPort, hardwareId),
  );
  const reportedFriendlyName = device?.properties?.friendly_name?.trim();
  const name = reportedFriendlyName || existing?.name || deviceLabelOf(device);
  const list = current.filter((d) => d !== existing);
  list.unshift({
    host,
    connectPort,
    name,
    deviceType: device?.device_type ?? existing?.deviceType ?? "unknown",
    ...(hardwareId || existing?.hardwareId
      ? { hardwareId: hardwareId ?? existing?.hardwareId }
      : {}),
    lastUsed: new Date().toISOString(),
  });
  write(list);
}

export function forgetDevice(host: string, connectPort: number): void {
  const current = listSavedDevices();
  const target = current.find(
    (d) => d.host === host && d.connectPort === connectPort,
  );
  if (!target) return;
  if (target.hardwareId) {
    write(current.filter((d) => d.hardwareId !== target.hardwareId));
    return;
  }
  write(
    current.filter(
      (d) =>
        d.hardwareId !== undefined ||
        d.host !== target.host ||
        d.connectPort !== target.connectPort,
    ),
  );
}

/// Whether the app may dial the single saved TV on launch without being
/// asked. Set by an explicit successful connect, cleared by an explicit
/// disconnect, and persisted so a process kill can't revive a connection the
/// user deliberately ended.
export function autoConnectEnabled(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "0";
  } catch {
    return true;
  }
}

export function setAutoConnect(enabled: boolean): void {
  try {
    localStorage.setItem(AUTO_KEY, enabled ? "1" : "0");
  } catch {
    // Storage unavailable — worst case we ask instead of auto-dialing.
  }
}

/// Cached friendly name for the TV at `host`. When the live TV reports a
/// hardware id, a cached row for that host is only trusted if it agrees, so a
/// reused IP can never show another TV's name.
export function cachedDeviceName(host: string, hardwareId?: string): string | null {
  if (!host) return null;
  const rows = listSavedDevices().filter((d) => d.host === host);
  const normalizedId = hardwareId?.trim();
  if (normalizedId && normalizedId !== "unknown") {
    return rows.find((d) => d.hardwareId === normalizedId)?.name ?? null;
  }
  if (rows.length === 0) return null;
  const identities = new Set(rows.map((d) => d.hardwareId ?? ""));
  return identities.size === 1 ? rows[0].name : null;
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
